-- Migration 0004 - the content layer: parsed elements, enrichment, vectors.
--
-- Workshop 002. Two ideas carry this file:
--
--   * elements are keyed by (blob_sha, parser_version). Parsing is cheap and
--     deterministic, so the same bytes read by the same parser always yield the
--     same tree - which is why an upsert here can never leave a stale row
--     behind, and why re-scanning is always safe.
--   * enrichment is keyed by raw_hash, NEVER by element id (decision D2). The
--     same function body on forty branches is summarised and embedded ONCE, and
--     "this element is dirty" is expressed as a MISSING row rather than a
--     stored flag that can drift. A model bump is a new model_key, so the old
--     rows are untouched and rollback is instant.
--
-- Decision D7: no foreign keys from smart_content or embeddings_* to elements.
-- Enrichment outlives any one parse - a parser_version bump re-mints every
-- element row while the expensive derived content stays valid. Collection is
-- explicit (D8: a `prune` job in a later plan), never a cascade.

-- ═══ ELEMENTS ════════════════════════════════════════════════════════════
--
-- 0001's `elements` was labelled in its own header as "the exemplar, not the
-- schema", and it is not liftable into this shape: it has no parser_version, no
-- parent link, no raw_hash and no enrich verdict, so three of the columns below
-- could only be backfilled by inventing them. It is dropped rather than
-- pretended over. That is safe for a reason worth stating: this table is a
-- DERIVED CACHE. Every row in it is reproducible by re-scanning the blob it
-- came from, so the cost of dropping it is one re-scan, not lost data.
--
-- 0002 (the kind-spelling fix) still matters and still runs first: a database
-- migrated between 0002 and 0004 needs it.
DROP TABLE IF EXISTS elements;

CREATE TABLE elements (
    id             BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- Content address of the whole file (PRD req 5).
    blob_sha       TEXT   NOT NULL,
    -- Which parser produced this tree. Bumping it re-mints elements without
    -- touching enrichment, because enrichment is keyed by raw_hash (D2).
    parser_version TEXT   NOT NULL,
    -- NULL for the file root. The tree shape 0001 could not express.
    parent_id      BIGINT REFERENCES elements(id) ON DELETE CASCADE,
    kind           TEXT   NOT NULL,
    -- The grammar's own node kind, verbatim (`impl_item`, `class_definition`),
    -- or for a file root, the language it was read as.
    subkind        TEXT   NOT NULL DEFAULT '',
    -- The declaration's own short name: `scan`, not `Indexer::scan`.
    name           TEXT   NOT NULL,
    -- Stable identity: `src/foo.rs::Indexer::scan`. Independent of line
    -- numbers, so an element keeps its address when code above it moves.
    address        TEXT   NOT NULL,
    span_start     INT    NOT NULL,
    span_end       INT    NOT NULL,
    sibling_order  INT    NOT NULL,
    -- Inline, per workshop 002's open question 2 (fs2 precedent): a query
    -- resolves content without needing repo access.
    raw_text       TEXT   NOT NULL,
    -- sha-256 of raw_text. THE dirtiness key, and the join key for everything
    -- expensive below.
    raw_hash       TEXT   NOT NULL,
    -- Decision D5: the scanner's injected-policy verdict, recorded once at
    -- scan time. Queue and backfill read this flag rather than re-deriving the
    -- policy, so the policy lives in exactly one place.
    enrich         BOOLEAN NOT NULL,

    -- `address` alone is NOT unique, and that is deliberate in the scanner:
    -- `struct Rect` and `impl Rect` are two elements sharing one address, so
    -- what identifies a node is (address, span_start). Keying on address alone
    -- would silently collapse the pair into one row on every scan.
    UNIQUE (blob_sha, parser_version, address, span_start),

    CONSTRAINT elements_kind_known
        CHECK (kind IN ('file', 'container', 'function', 'section')),
    -- Kept under 0001's name: the constraint is the same contract, and the
    -- test that proves it bites cites this name.
    CONSTRAINT elements_span_ordered CHECK (span_end >= span_start)
);

-- The enrichment join, and the reconciler's sweep (D6).
CREATE INDEX elements_raw_hash_idx ON elements (raw_hash);
-- "have I already parsed this blob with this parser?" - the scan-job fast path.
CREATE INDEX elements_blob_parser_idx ON elements (blob_sha, parser_version);
-- Rebuilding a tree walks parents to children.
CREATE INDEX elements_parent_idx ON elements (parent_id);

-- ═══ SMART CONTENT ═══════════════════════════════════════════════════════
--
-- fs2's LLM layer, content-addressed. Dedupes across branches, worktrees and
-- repositories by construction.
CREATE TABLE smart_content (
    -- Which raw text this describes. Not an element id (D2).
    raw_hash    TEXT        NOT NULL,
    -- "<model>@<prompt_version>", from the config registry.
    model_key   TEXT        NOT NULL,
    text        TEXT        NOT NULL,
    -- sha-256 of `text`, written by the store using fs3_core::content_hash -
    -- the one hash function in fs3.
    --
    -- Workshop 002 defines embeddings.source_hash for a summary vector as
    -- "sha256(smart text)", which nothing else in the sketch records: a
    -- nearest-neighbour hit on a summary would have had no way back to the
    -- element it describes. This column is that way back. It is computed in
    -- Rust rather than by a generated column so there is exactly one hash
    -- implementation in the system, not a Postgres one beside it.
    text_hash   TEXT        NOT NULL,
    -- PRD req 36: 1-5 concept tags, and the band is enforced here as well as
    -- in the domain type, because the database is the last line.
    tags        TEXT[]      NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (raw_hash, model_key),

    CONSTRAINT smart_content_tag_band
        CHECK (cardinality(tags) BETWEEN 1 AND 5)
);

-- Resolving a summary embedding back to its element.
CREATE INDEX smart_content_text_hash_idx ON smart_content (text_hash);

-- ═══ EMBEDDINGS ══════════════════════════════════════════════════════════
--
-- Decision D3: per-dimension tables. An HNSW index requires a typed dimension,
-- so `vector` without one cannot be indexed - a single untyped table would
-- have made every similarity query a sequential scan. A model of another width
-- arrives as another migration creating embeddings_<dim>, which the daemon
-- (the single writer) owns.
--
-- Only embeddings_1024 exists today, because only 1024-wide models are
-- configured today. Adding the table before a model needs it would be an index
-- with nothing in it.
CREATE TABLE embeddings_1024 (
    -- raw_hash for a raw-content vector, smart_content.text_hash for a summary
    -- vector. `source_kind` says which, and is what makes the join unambiguous.
    source_hash TEXT        NOT NULL,
    source_kind TEXT        NOT NULL,
    -- The EMBEDDING model's key. Deliberately a different namespace from
    -- smart_content.model_key, which names the summarising model: the two are
    -- different models and are never compared.
    model_key   TEXT        NOT NULL,
    vector      vector(1024) NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),

    PRIMARY KEY (source_hash, source_kind, model_key),

    CONSTRAINT embeddings_1024_source_kind_known
        CHECK (source_kind IN ('raw', 'smart'))
);

-- Cosine, matching the `<=>` operator the similarity query uses. An index
-- built for another operator class is silently not used.
CREATE INDEX embeddings_1024_vector_idx
    ON embeddings_1024 USING hnsw (vector vector_cosine_ops);
