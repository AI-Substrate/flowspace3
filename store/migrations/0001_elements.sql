-- Migration 0001 - the exemplar, not the schema.
--
-- Plan 001 ships exactly enough table to prove the migration + round-trip
-- shape. The real content/ref layer design is workshop material for plan 002
-- (blob-keyed content, (blob, model, prompt-version) derived rows, tag storage
-- for PRD req 36, GC). Do not grow this file - add 0002.

-- PRD req 4: the store is Postgres *with pgvector*. Creating the extension here
-- means a stack without it fails at migrate time with a clear error, rather
-- than at the first embedding write.
CREATE EXTENSION IF NOT EXISTS vector;

CREATE TABLE IF NOT EXISTS elements (
    -- Content address (PRD req 5): the git blob SHA of the file's bytes.
    blob            TEXT   NOT NULL,
    -- Repo-relative path the element was found at.
    path            TEXT   NOT NULL,
    -- Nested name, e.g. `geometry.Rect.new` or `Main Title > Section One`.
    qualified_name  TEXT   NOT NULL,
    -- Raw tree-sitter node kind, kept verbatim.
    ts_kind         TEXT   NOT NULL,
    -- Universal category: callable | type | section.
    kind            TEXT   NOT NULL,
    start_line      INTEGER NOT NULL,
    end_line        INTEGER NOT NULL,
    body            TEXT   NOT NULL,
    -- POC learning L8: recorded, never used to reject a file.
    has_error       BOOLEAN NOT NULL DEFAULT FALSE,

    PRIMARY KEY (blob, qualified_name, start_line),

    CONSTRAINT elements_kind_known CHECK (kind IN ('callable', 'type', 'section')),
    CONSTRAINT elements_span_ordered CHECK (end_line >= start_line)
);

CREATE INDEX IF NOT EXISTS elements_path_idx ON elements (path);
